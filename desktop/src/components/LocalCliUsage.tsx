import React from "react";

export interface LocalCliUsageRow {
  cli: "Claude" | "Codex" | "Qoder";
  version: string | null;
  installed: boolean;
  runs: number;
  sessions: number;
  tokens: number;
  lastUsed: string;
}

function formatTokens(tokens: number): string {
  if (tokens >= 1000) return `${(tokens / 1000).toFixed(1)}K`;
  return String(tokens);
}

function percent(value: number, max: number): string {
  if (max <= 0) return "0%";
  return `${Math.max(4, Math.round((value / max) * 100))}%`;
}

export function LocalCliUsage({ rows }: { rows: LocalCliUsageRow[] }) {
  const maxRuns = Math.max(...rows.map((row) => row.runs), 0);
  const maxSessions = Math.max(...rows.map((row) => row.sessions), 0);
  const maxTokens = Math.max(...rows.map((row) => row.tokens), 0);

  return (
    <div className="border border-app-border bg-app-sidebar">
      <div className="h-8 px-3 border-b border-app-border flex items-center justify-between">
        <span className="text-2xs font-mono text-app-text uppercase tracking-[0.14em]">Local CLI Usage</span>
        <span className="text-2xs font-mono text-app-text-muted">版本 / 使用 / 会话 / Token / 最近</span>
      </div>
      <div className="p-2 space-y-1.5">
        {rows.map((row) => (
          <div key={row.cli} className="grid grid-cols-[92px_1fr] gap-2 border border-app-border-light bg-[var(--app-input)] p-2">
            <div className="min-w-0">
              <div className="flex items-center gap-1.5 text-xs font-mono text-app-text">
                <span className={`w-1.5 h-1.5 rounded-full ${row.installed ? "bg-app-green" : "bg-app-amber"}`} />
                {row.cli}
              </div>
              <div className="text-2xs font-mono text-app-text-muted truncate">
                {row.installed ? row.version : "未安装"}
              </div>
            </div>
            <div className="grid grid-cols-4 gap-1.5">
              {[
                ["使用", row.runs, percent(row.runs, maxRuns)],
                ["会话", row.sessions, percent(row.sessions, maxSessions)],
                ["Tokens", formatTokens(row.tokens), percent(row.tokens, maxTokens)],
                ["最近", row.lastUsed, row.installed ? "100%" : "0%"],
              ].map(([label, value, width]) => (
                <div key={label} className="min-w-0 border border-app-border bg-app-bg px-1.5 py-1">
                  <div className="flex justify-between gap-1 text-2xs font-mono">
                    <span className="text-app-text">{value}</span>
                    <span className="text-app-text-muted">{label}</span>
                  </div>
                  <div className="h-[3px] bg-[var(--app-border)] mt-1">
                    <div className="h-full bg-app-accent" style={{ width: String(width) }} />
                  </div>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
