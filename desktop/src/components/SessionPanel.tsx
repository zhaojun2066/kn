import { useCallback, useMemo, useState } from "react";
import { CheckSquare, ChevronDown, ChevronRight, Globe, Monitor, Square, TerminalSquare, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { ConfirmDialog } from "./ConfirmDialog";
import type { AgentSession, AgentState } from "../hooks/useAgent";
import { buildKillSessionIpcArgs } from "./AgentPanel";

type SessionTab = "local" | "remote";

export interface SessionPanelProps {
  agent: AgentState;
  initialTab: SessionTab;
  onClose: () => void;
  onOpenRemoteSession: (session: AgentSession) => void;
  canOpenLocalRelaySession: (session: AgentSession) => boolean;
}

function basename(path: string) {
  return path.split("/").filter(Boolean).pop() || path;
}

function SessionRow({ session, checked, onCheck, onOpen }: { session: AgentSession; checked: boolean; onCheck: () => void; onOpen?: () => void }) {
  const [expanded, setExpanded] = useState(false);
  const remote = session.remote_enabled;
  return <div className="text-xs font-mono border-b border-app-border-light last:border-0">
    <div className="flex items-center gap-2 px-3 py-2 hover:bg-[var(--app-hover)]">
      <button type="button" onClick={onCheck} aria-label={`选择 ${basename(session.cwd)}`} className="text-app-text-dim hover:text-app-accent">
        {checked ? <CheckSquare size={13} className="text-app-accent" /> : <Square size={13} />}
      </button>
      <button type="button" onClick={() => setExpanded((value) => !value)} className="flex flex-1 min-w-0 items-center gap-2 text-left">
        {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
        {remote ? <Globe size={13} className="text-app-text-dim" /> : <Monitor size={13} className="text-app-text-dim" />}
        <span className="min-w-0 flex-1"><span className="block truncate text-app-text">{basename(session.cwd)}</span><span className="block truncate text-[10px] text-app-text-muted">{session.tool}@{session.profile || "default"} · {session.nid}</span></span>
      </button>
      <span className={session.status === "running" ? "w-1.5 h-1.5 rounded-full bg-app-green" : "w-1.5 h-1.5 rounded-full bg-app-text-muted"} title={session.status} />
      {remote && onOpen && <button type="button" onClick={onOpen} className="p-1 text-app-text-dim hover:text-app-accent" title="打开远程会话"><TerminalSquare size={13} /></button>}
    </div>
    {expanded && <div className="ml-10 px-3 pb-2 text-[10px] leading-relaxed text-app-text-muted"><div>路径：{session.cwd}</div><div>创建时间：{session.created_at}</div><div>会话 ID：{session.nid}</div></div>}
  </div>;
}

export function SessionPanel({ agent, initialTab, onClose, onOpenRemoteSession, canOpenLocalRelaySession }: SessionPanelProps) {
  const [tab, setTab] = useState<SessionTab>(initialTab);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState<{ kind: "enable" | "kill"; targets: AgentSession[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [processing, setProcessing] = useState(false);
  const running = useMemo(() => agent.sessions.filter((session) => session.status === "running"), [agent.sessions]);
  const local = useMemo(() => running.filter((session) => !session.remote_enabled), [running]);
  const remote = useMemo(() => running.filter((session) => session.remote_enabled), [running]);
  const visible = tab === "local" ? local : remote;
  const remoteAccessAllowed = agent.agentStatus?.remote_access?.allowed !== false;

  const toggle = useCallback((nid: string) => setSelected((previous) => { const next = new Set(previous); next.has(nid) ? next.delete(nid) : next.add(nid); return next; }), []);
  const selectAll = useCallback(() => setSelected((previous) => previous.size === visible.length ? new Set() : new Set(visible.map((session) => session.nid))), [visible]);
  const run = useCallback(async (kind: "enable" | "disable" | "kill", targets: AgentSession[]) => {
    if (processing) return;
    setProcessing(true);
    let failures = 0;
    try {
      for (const session of targets) {
        try {
          if (kind === "kill") await invoke("agent_ipc", buildKillSessionIpcArgs(session));
          else await invoke("agent_ipc", { method: "set_remote_enabled", params: { nid: session.nid, enabled: kind === "enable" } });
        } catch (reason) {
          failures += 1;
          setError(kind === "enable" ? `开启远程会话失败：${String(reason)}` : `${failures} 个会话操作失败`);
          if (kind === "enable") break;
        }
      }
      setSelected(new Set());
      await agent.fetchSessions();
    } finally {
      setProcessing(false);
    }
  }, [agent, processing]);
  const targets = visible.filter((session) => selected.has(session.nid));
  const allSelected = visible.length > 0 && selected.size === visible.length;
  const changeTab = (next: SessionTab) => { setTab(next); setSelected(new Set()); setError(null); };

  return <>
    <div className="fixed inset-0 z-[120] flex items-end justify-end px-4 pb-[26px]" onClick={onClose}>
      <section className="app-dialog-panel bg-app-panel border border-app-border w-[60vw] min-w-[380px] max-w-[480px] max-h-[min(560px,calc(100vh-3rem))] flex flex-col animate-[slideUp_150ms_ease-out]" onClick={(event) => event.stopPropagation()} aria-label="会话">
        <header className="flex shrink-0 items-center justify-between px-4 py-3 border-b border-app-border"><div className="flex items-center gap-2"><TerminalSquare size={16} className="text-app-accent" /><div><h2 className="text-sm font-semibold text-app-text">会话</h2><p className="text-[10px] text-app-text-muted">管理这台 Mac 上的本地与手机远程会话</p></div></div><button type="button" onClick={onClose} aria-label="关闭会话" className="p-1 text-app-text-dim hover:text-app-text"><X size={15} /></button></header>
        <div className="flex border-b border-app-border">
          {(["local", "remote"] as const).map((item) => { const count = item === "local" ? local.length : remote.length; const Icon = item === "local" ? Monitor : Globe; return <button key={item} type="button" onClick={() => changeTab(item)} className={`flex-1 py-2 text-xs font-medium flex justify-center items-center gap-2 ${tab === item ? "text-app-accent border-b-2 border-app-accent -mb-px" : "text-app-text-dim hover:text-app-text"}`}><Icon size={13} />{item === "local" ? "本地会话" : "远程会话"}<span className="text-app-text-muted">{count}</span></button>; })}
        </div>
        <div className="flex items-center justify-between px-3 py-2 border-b border-app-border bg-[var(--app-subtle)] text-xs"><button type="button" disabled={processing} onClick={selectAll} className="flex items-center gap-1.5 text-app-text-dim hover:text-app-text disabled:opacity-40">{allSelected ? <CheckSquare size={13} className="text-app-accent" /> : <Square size={13} />}全选</button><div className="flex gap-2">{tab === "local" ? <button type="button" disabled={!targets.length || !remoteAccessAllowed || processing} title={!remoteAccessAllowed ? agent.agentStatus?.remote_access?.message : undefined} onClick={() => { if (remote.length + targets.length > 10) setError("已达到远程控制上限（10 个）"); else setConfirm({ kind: "enable", targets }); }} className="px-2 py-1 border border-app-accent/50 text-app-accent disabled:opacity-40">{processing ? "处理中…" : "开启远程"}</button> : <button type="button" disabled={!targets.length || processing} onClick={() => void run("disable", targets)} className="px-2 py-1 border border-app-border text-app-text-dim disabled:opacity-40">{processing ? "处理中…" : "关闭远程"}</button>}<button type="button" disabled={!targets.length || processing} onClick={() => setConfirm({ kind: "kill", targets })} className="px-2 py-1 border border-app-red/50 text-app-red disabled:opacity-40">终止进程</button></div></div>
        {error && <div role="alert" className="mx-3 mt-3 px-3 py-2 text-xs text-app-red bg-app-red/10">{error}</div>}
        <div className="min-h-0 overflow-y-auto">{visible.length ? visible.map((session) => <SessionRow key={session.nid} session={session} checked={selected.has(session.nid)} onCheck={() => toggle(session.nid)} onOpen={session.remote_enabled && (session.kind === "Native" || canOpenLocalRelaySession(session)) ? () => { onOpenRemoteSession(session); onClose(); } : undefined} />) : <div className="px-6 py-12 text-center text-sm text-app-text-muted">{tab === "local" ? "暂无本地会话" : "暂无远程会话"}</div>}</div>
      </section>
    </div>
    <ConfirmDialog open={confirm !== null} title={confirm?.kind === "enable" ? "开启远程会话" : "终止进程"} message={confirm?.kind === "enable" ? `将向手机共享 ${confirm.targets.length} 个选中终端的输出，并允许手机输入命令。` : `确定要终止 ${confirm?.targets.length ?? 0} 个终端进程吗？此操作不可撤销。`} confirmLabel={confirm?.kind === "enable" ? "确认开启" : "终止"} variant={confirm?.kind === "enable" ? "primary" : "danger"} onConfirm={() => { if (confirm) void run(confirm.kind === "enable" ? "enable" : "kill", confirm.targets); setConfirm(null); }} onCancel={() => setConfirm(null)} />
  </>;
}
