import { ChevronDown, ChevronRight, RefreshCw } from "lucide-react";
import { useState } from "react";
import { healthToolLabel, type AgentHealthSnapshot } from "../lib/healthReport";

interface AgentHealthSectionProps {
  health: AgentHealthSnapshot | null;
  isLoading: boolean;
  error: string | null;
  onRefresh: () => void;
}

export function AgentHealthSection({
  health,
  isLoading,
  error,
  onRefresh,
}: AgentHealthSectionProps) {
  const [isExpanded, setIsExpanded] = useState(true);

  return (
    <section className="border border-app-border bg-[var(--app-cmd-bg)]">
      <div className="flex items-center justify-between gap-2 px-2.5 py-2 border-b border-app-border">
        <button
          type="button"
          onClick={() => setIsExpanded((expanded) => !expanded)}
          aria-expanded={isExpanded}
          className="min-w-0 flex flex-1 items-center gap-1.5 px-1 py-1 text-left text-xs font-mono text-app-text-dim hover:text-app-text transition-colors"
          title={isExpanded ? "收起设备健康" : "展开设备健康"}
        >
          {isExpanded ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          <span>设备健康</span>
          <span className="truncate text-[10px] text-app-text-muted">
            {isLoading && !isExpanded ? "正在检查…" : "仅检查本机连接与工具能力"}
          </span>
        </button>
        <button
          type="button"
          onClick={onRefresh}
          disabled={isLoading}
          className="flex shrink-0 items-center gap-1 text-[11px] text-app-text-dim hover:text-app-text disabled:opacity-50"
        >
          <RefreshCw size={13} className={isLoading ? "animate-spin" : ""} />
          {isLoading ? "正在刷新…" : "刷新"}
        </button>
      </div>

      {isExpanded && (
        <div className="px-2.5 py-2 space-y-1.5 text-[11px] font-mono">
          {isLoading && (
            <div role="status" className="text-app-text-muted">
              正在检查…
            </div>
          )}
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
            </>
          )}
        </div>
      )}
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
