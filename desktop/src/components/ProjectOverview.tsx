import React from "react";
import type { ProjectInfo, SessionInfo } from "../lib/types";
import { LocalCliUsage, type LocalCliUsageRow } from "./LocalCliUsage";

interface ProjectOverviewProps {
  project: ProjectInfo;
  sessions: SessionInfo[];
  cliUsageRows: LocalCliUsageRow[];
  onRunDefault: () => void;
  onChangeDefaultProfile: () => void;
}

export function ProjectOverview({
  project,
  sessions,
  cliUsageRows,
  onRunDefault,
  onChangeDefaultProfile,
}: ProjectOverviewProps) {
  return (
    <div className="flex-1 min-h-0 overflow-y-auto p-4 grid grid-cols-[minmax(0,1fr)_420px] gap-4 bg-[var(--app-bg)]">
      <section className="space-y-3 min-w-0">
        <div className="border border-app-border bg-app-sidebar p-3">
          <div className="text-sm font-mono text-app-text">{project.name}</div>
          <div className="text-2xs font-mono text-app-text-muted truncate mt-1">{project.path}</div>
          <div className="flex items-center gap-2 mt-3">
            <button onClick={onRunDefault} className="px-2 py-1 text-xs font-mono bg-app-accent text-[var(--app-bg)]">
              Run default
            </button>
            <button onClick={onChangeDefaultProfile} className="px-2 py-1 text-xs font-mono border border-app-border text-app-text-dim">
              默认 Profile: {project.defaultProfile || "未设置"}
            </button>
          </div>
        </div>
        <div className="border border-app-border bg-app-sidebar">
          <div className="h-8 px-3 border-b border-app-border flex items-center text-2xs font-mono text-app-text uppercase tracking-[0.14em]">
            Recent Sessions
          </div>
          <div className="p-2 space-y-1">
            {sessions.slice(0, 5).map((session) => (
              <div key={session.sessionId} className="text-xs font-mono text-app-text-dim border border-app-border-light p-2">
                {session.title}
              </div>
            ))}
            {sessions.length === 0 && (
              <div className="text-xs font-mono text-app-text-muted p-2">暂无会话</div>
            )}
          </div>
        </div>
      </section>
      <aside className="space-y-3 min-w-0">
        <LocalCliUsage rows={cliUsageRows} />
        <div className="border border-app-border bg-app-sidebar p-3">
          <div className="text-2xs font-mono text-app-text uppercase tracking-[0.14em]">Recommended Actions</div>
          <div className="text-xs font-mono text-app-text-muted mt-2">
            检查默认 Profile、项目级 Hooks、常用 Skills 和资源冲突。
          </div>
        </div>
      </aside>
    </div>
  );
}
