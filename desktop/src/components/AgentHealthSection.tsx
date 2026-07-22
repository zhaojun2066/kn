import { Copy, RefreshCw } from "lucide-react";
import { healthToolLabel, type AgentHealthSnapshot } from "../lib/healthReport";

interface AgentHealthSectionProps {
  health: AgentHealthSnapshot | null;
  isLoading: boolean;
  error: string | null;
  copied: boolean;
  onRefresh: () => void;
  onCopy: () => void;
}

export function AgentHealthSection({
  health,
  isLoading,
  error,
  copied,
  onRefresh,
  onCopy,
}: AgentHealthSectionProps) {
  return (
    <section className="border border-app-border bg-[var(--app-cmd-bg)]">
      <div className="flex items-center justify-between gap-2 px-2.5 py-2 border-b border-app-border">
        <div>
          <div className="text-xs font-mono text-app-text">设备健康</div>
          <div className="text-[10px] font-mono text-app-text-muted">仅检查本机连接与工具能力</div>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={onRefresh}
            disabled={isLoading}
            className="p-1 text-app-text-dim hover:text-app-text disabled:opacity-40"
            title="重新检查"
          >
            <RefreshCw size={13} className={isLoading ? "animate-spin" : ""} />
          </button>
          <button
            onClick={onCopy}
            disabled={!health}
            className="p-1 text-app-text-dim hover:text-app-text disabled:opacity-40"
            title="复制脱敏诊断"
          >
            <Copy size={13} />
          </button>
        </div>
      </div>

      <div className="px-2.5 py-2 space-y-1.5 text-[11px] font-mono">
        {isLoading && !health && <div className="text-app-text-muted">正在检查…</div>}
        {error && <div className="text-amber-400">{error}</div>}
        {health && (
          <>
            <div className="flex justify-between gap-3 text-app-text-muted">
              <span>连接</span>
              <span className="text-app-text">{connectionLabel(health.connection.state)}</span>
            </div>
            <div className="flex justify-between gap-3 text-app-text-muted">
              <span>Agent</span>
              <span className="text-app-text">v{health.agent.version} · {health.agent.environment === "development" ? "开发" : "生产"}</span>
            </div>
            {health.tools.map((tool) => (
              <div key={tool.name} className="flex justify-between gap-3 text-app-text-muted">
                <span>{healthToolLabel(tool).split(" · ")[0]}</span>
                <span className={tool.state === "available" ? "text-emerald-400" : "text-amber-400"}>
                  {healthToolLabel(tool).split(" · ")[1]}
                </span>
              </div>
            ))}
            <div className="pt-1 text-[10px] text-app-text-dim">
              {copied ? "已复制脱敏诊断" : "不包含路径、命令、地址或凭证"}
            </div>
          </>
        )}
      </div>
    </section>
  );
}

function connectionLabel(state: AgentHealthSnapshot["connection"]["state"]): string {
  const labels = {
    connected: "已连接",
    reconnecting: "重连中",
    notReady: "未准备好",
    unavailable: "暂时不可用",
  };
  return labels[state];
}
