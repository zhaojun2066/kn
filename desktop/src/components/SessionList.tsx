import React, { useState, useMemo, useCallback } from "react";
import { CLIIcon } from "./common/CLIIcon";
import { Circle, Loader, ChevronDown } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { CLI_TYPES, type CliKind, type SessionInfo } from "../lib/types";
import { relativeTime } from "../lib/time-utils";

interface SessionListProps {
  sessions: SessionInfo[];
  loading: boolean;
  onResume: (session: SessionInfo) => void;
}

export function SessionList({ sessions, loading, onResume }: SessionListProps) {
  const [cliFilter, setCliFilter] = useState<CliKind | "all">("all");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [preview, setPreview] = useState<string[]>([]);
  const [previewLoading, setPreviewLoading] = useState(false);

  const filtered = useMemo(() => {
    if (cliFilter === "all") return sessions;
    return sessions.filter((s) => s.cli === cliFilter);
  }, [sessions, cliFilter]);

  const togglePreview = useCallback(async (s: SessionInfo) => {
    if (expandedId === s.sessionId) {
      setExpandedId(null);
      setPreview([]);
    } else {
      setExpandedId(s.sessionId);
      setPreviewLoading(true);
      try {
        const msgs: string[] = await invoke("read_session_preview", {
          cli: s.cli,
          projectPath: s.projectPath,
          sessionId: s.sessionId,
        });
        setPreview(msgs);
      } catch {
        setPreview([]);
      } finally {
        setPreviewLoading(false);
      }
    }
  }, [expandedId]);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12 gap-2 text-[var(--app-text-muted)]">
        <Loader size={14} className="animate-spin" />
        <span className="text-2xs font-mono">扫描会话...</span>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      {/* CLI filter bar */}
      <div className="flex items-center gap-1 px-3 py-2 border-b border-[var(--app-border)] shrink-0">
        <span className="text-3xs text-[var(--app-text-muted)] font-mono mr-1">筛选:</span>
        <button
          onClick={() => setCliFilter("all")}
          className={`px-2 py-0.5 text-3xs font-mono transition-colors cursor-pointer
            ${cliFilter === "all"
              ? "text-[var(--app-accent)] bg-[var(--app-selected)]"
              : "text-[var(--app-text-dim)] hover:text-[var(--app-text)]"
            }`}
        >
          全部
        </button>
        {CLI_TYPES.map((t) => (
          <button
            key={t.id}
            onClick={() => setCliFilter(t.id)}
            className={`flex items-center gap-1 px-2 py-0.5 text-3xs font-mono transition-colors cursor-pointer
              ${cliFilter === t.id
                ? "text-[var(--app-accent)] bg-[var(--app-selected)]"
                : "text-[var(--app-text-dim)] hover:text-[var(--app-text)]"
              }`}
          >
            <CLIIcon type={t.id} size={11} />
            {t.label}
          </button>
        ))}
      </div>

      {/* Session list */}
      {filtered.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-12 gap-2 text-[var(--app-text-muted)] flex-1">
          <span className="text-xs font-mono">
            {sessions.length === 0 ? "暂无会话" : "无匹配会话"}
          </span>
          <span className="text-3xs font-mono">
            {sessions.length === 0 ? "在终端中运行 AI CLI 后刷新" : "尝试切换筛选条件"}
          </span>
        </div>
      ) : (
        <div className="py-1 flex-1 overflow-y-auto">
          {filtered.map((s) => (
            <div
              key={s.sessionId}
              onClick={() => togglePreview(s)}
              className={`group flex flex-col mx-1 my-px cursor-pointer
                transition-all duration-fast
                ${expandedId === s.sessionId
                  ? "bg-[var(--app-selected)]/50 border-l-[3px] border-l-[var(--app-accent)]"
                  : "border-l-[3px] border-l-transparent hover:bg-[var(--app-hover)]"
                }`}
            >
              <div className="flex items-center gap-2.5 px-2.5 py-2 text-[var(--app-text)]">
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

              <button
                onClick={(e) => { e.stopPropagation(); onResume(s); }}
                className="shrink-0 px-2 py-0.5 text-3xs font-mono
                  text-[var(--app-accent)] border border-[var(--app-accent-dim)]
                  hover:bg-[var(--app-accent)] hover:text-[var(--app-bg)]
                  opacity-0 group-hover:opacity-100 transition-all
                  cursor-pointer outline-none"
              >
                恢复
              </button>
              <ChevronDown
                size={11}
                className={`shrink-0 text-[var(--app-text-muted)]/30 transition-transform duration-150 ${
                  expandedId === s.sessionId ? "rotate-180" : ""
                }`}
              />
            </div>

            {/* Preview: expanded session summary */}
            {expandedId === s.sessionId && (
              <div
                className="px-2.5 pb-2.5 pt-0 border-t border-[var(--app-border)]/30"
                onClick={(e) => e.stopPropagation()}
              >
                {previewLoading ? (
                  <div className="flex items-center gap-2 py-3 text-[10px] text-[var(--app-text-muted)] font-mono">
                    <Loader size={10} className="animate-spin" />
                    加载摘要...
                  </div>
                ) : preview.length > 0 ? (
                  <div className="flex flex-col gap-1.5 pt-2">
                    {preview.map((msg, i) => (
                      <div
                        key={i}
                        className={`text-[10px] font-mono leading-relaxed px-2 py-1 ${
                          i % 2 === 0
                            ? "text-[var(--app-text-dim)] border-l-2 border-[var(--app-accent)]/30"
                            : "text-[var(--app-text-muted)] border-l-2 border-[var(--app-purple)]/20"
                        }`}
                      >
                        {msg}
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="py-3 text-[10px] text-[var(--app-text-muted)]/40 font-mono text-center">
                    无可用摘要
                  </div>
                )}
              </div>
            )}
          </div>
          ))}
        </div>
      )}
    </div>
  );
}
