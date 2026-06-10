import React from "react";
import { CLIIcon } from "./common/CLIIcon";
import { Circle, Loader } from "lucide-react";
import type { SessionInfo } from "../lib/types";

interface SessionListProps {
  sessions: SessionInfo[];
  loading: boolean;
  onResume: (session: SessionInfo) => void;
}

function relativeTime(ts: number): string {
  const now = Date.now();
  const diff = now - ts;
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return "刚刚";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    const date = new Date(ts);
    const hh = date.getHours().toString().padStart(2, "0");
    const mm = date.getMinutes().toString().padStart(2, "0");
    return `今天 ${hh}:${mm}`;
  }
  const days = Math.floor(hours / 24);
  if (days === 1) return "昨天";
  if (days < 7) return `${days}天前`;
  const date = new Date(ts);
  const m = (date.getMonth() + 1).toString().padStart(2, "0");
  const d = date.getDate().toString().padStart(2, "0");
  return `${m}-${d}`;
}

export function SessionList({ sessions, loading, onResume }: SessionListProps) {
  if (loading) {
    return (
      <div className="flex items-center justify-center py-12 gap-2 text-[var(--app-text-muted)]">
        <Loader size={14} className="animate-spin" />
        <span className="text-2xs font-mono">扫描会话...</span>
      </div>
    );
  }

  if (sessions.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 gap-2 text-[var(--app-text-muted)]">
        <span className="text-xs font-mono">暂无会话</span>
        <span className="text-3xs font-mono">在终端中运行 AI CLI 后刷新</span>
      </div>
    );
  }

  return (
    <div className="py-1">
      {sessions.map((s) => (
        <div
          key={s.sessionId}
          onClick={() => onResume(s)}
          className="group flex items-center gap-2.5 mx-1 my-px px-2.5 py-2
            cursor-pointer text-[var(--app-text)] hover:bg-[var(--app-hover)]
            border-l-[3px] border-l-transparent hover:border-l-[var(--app-accent)]
            transition-all duration-fast"
        >
          <Circle
            size={7}
            className={`shrink-0 ${
              s.status === "active"
                ? "fill-[var(--app-green)] text-[var(--app-green)]"
                : "fill-transparent text-[var(--app-text-muted)]"
            }`}
          />

          <CLIIcon type={s.cli} size={14} />

          <div className="flex-1 min-w-0">
            <div className="text-xs font-mono truncate">
              {s.title}
            </div>
            <div className="flex items-center gap-1.5 mt-0.5">
              <span className="text-3xs text-[var(--app-text-muted)] font-mono">
                {s.cli}
              </span>
              {s.profile && (
                <>
                  <span className="text-3xs text-[var(--app-border)]">·</span>
                  <span className="text-3xs text-[var(--app-amber)] font-mono">
                    {s.profile}
                  </span>
                </>
              )}
              <span className="text-3xs text-[var(--app-border)]">·</span>
              <span className="text-3xs text-[var(--app-text-muted)] font-mono">
                {relativeTime(s.timestamp)}
              </span>
            </div>
          </div>

          <span className="text-3xs text-[var(--app-accent-dim)] opacity-0 group-hover:opacity-100 transition-opacity font-mono shrink-0">
            恢复 →
          </span>
        </div>
      ))}
    </div>
  );
}
