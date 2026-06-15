import React, { useState } from "react";
import { X, BarChart3, ChevronRight, ChevronDown } from "lucide-react";
import { useUsage } from "../hooks/useUsage";
import type { ProjectUsage } from "../lib/tauri-api";

type Period = "today" | "week" | "month";
const PERIOD_DAYS: Record<Period, number> = {
  today: 1,
  week: 7,
  month: 30,
};

type Dimension = "model" | "project";

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatDayLabel(date: string): string {
  return date.length >= 10 ? date.slice(5) : date;
}

/** Generate a display label for a project path, disambiguating when needed. */
function projectDisplayLabel(
  pu: ProjectUsage,
  allProjects: ProjectUsage[],
): string {
  // Use explicit project_name if available
  if (pu.project_name && pu.project_name !== "未关联项目") {
    const basename = pu.project_name;
    // Check for conflicts among other named projects
    const conflicts = allProjects.filter(
      (p) =>
        p.project_path !== pu.project_path &&
        p.project_name === basename &&
        p.project_name !== "未关联项目",
    );
    if (conflicts.length > 0 && pu.project_path) {
      // Show parent dir for disambiguation
      const parts = pu.project_path.replace(/\/+$/, "").split("/");
      const parent = parts.length >= 2 ? parts[parts.length - 2] : "";
      return parent ? `${parent}/${basename}` : basename;
    }
    return basename;
  }
  // Fallback: derive from project_path
  if (!pu.project_path) return pu.project_name ?? "未关联项目";
  const parts = pu.project_path.replace(/\/+$/, "").split("/");
  if (parts.length >= 2) {
    return `${parts[parts.length - 2]}/${parts[parts.length - 1]}`;
  }
  return parts[0] || pu.project_path;
}

/** Full path tooltip text */
function projectTooltip(pu: ProjectUsage): string {
  if (pu.project_path) return pu.project_path;
  return pu.project_name ?? "";
}

interface UsagePanelProps {
  open: boolean;
  onClose: () => void;
}

export function UsagePanel({ open, onClose }: UsagePanelProps) {
  const [period, setPeriod] = useState<Period>("week");
  const [dimension, setDimension] = useState<Dimension>("model");
  const [expandedProject, setExpandedProject] = useState<string | null>(null);
  const days = PERIOD_DAYS[period];
  const { summary, daily, projectUsage, loading, refresh } = useUsage({
    summaryDays: days,
    dailyDays: days,
  });

  if (!open) return null;

  const totalTokens = summary
    ? summary.total_tokens_in + summary.total_tokens_out
    : 0;
  const hasData = summary && totalTokens > 0;
  const hasProjectData = projectUsage.length > 0;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="bg-app-panel border border-app-border shadow-dialog w-[620px] max-h-[85vh] overflow-y-auto select-none animate-[scaleIn_150ms_ease-out]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border sticky top-0 bg-app-panel z-10">
          <div className="flex items-center gap-2">
            <BarChart3 size={15} className="text-app-accent" />
            <span className="text-sm font-mono text-app-text font-semibold">Token 用量</span>
          </div>
          <button onClick={onClose} className="p-0.5 text-app-text-dim hover:text-app-text transition-colors">
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="px-4 py-4 space-y-5">
          {/* Period tabs */}
          <div className="flex gap-1 bg-[var(--app-cmd-bg)] border border-app-border p-0.5 w-fit">
            {([["today", "今天"], ["week", "近 7 天"], ["month", "近 30 天"]] as const).map(([k, label]) => (
              <button
                key={k}
                onClick={() => setPeriod(k as Period)}
                className={`px-3 py-1 text-xs font-mono transition-colors ${
                  period === k
                    ? "bg-app-accent text-[var(--app-bg)]"
                    : "text-app-text-dim hover:text-app-text"
                }`}
              >
                {label}
              </button>
            ))}
          </div>

          {hasData ? (
            <>
              {/* Summary card */}
              <div className="border border-app-border bg-[var(--app-cmd-bg)] px-4 py-3 text-center">
                <div className="text-2xs text-app-text-muted font-mono uppercase tracking-wider mb-1">
                  Token 消耗
                </div>
                <div className="text-lg font-mono font-bold text-app-text">
                  {formatTokens(totalTokens)}
                </div>
                <div className="text-2xs text-app-text-muted font-mono mt-0.5">
                  入 {formatTokens(summary.total_tokens_in)} · 出 {formatTokens(summary.total_tokens_out)}
                </div>
              </div>

              {/* Dimension toggle */}
              <div className="flex gap-1 bg-[var(--app-cmd-bg)] border border-app-border p-0.5 w-fit">
                {([["model", "按模型"], ["project", "按项目"]] as const).map(([k, label]) => (
                  <button
                    key={k}
                    onClick={() => setDimension(k as Dimension)}
                    className={`px-3 py-1 text-xs font-mono transition-colors ${
                      dimension === k
                        ? "bg-app-accent text-[var(--app-bg)]"
                        : "text-app-text-dim hover:text-app-text"
                    }`}
                  >
                    {label}
                  </button>
                ))}
              </div>

              {/* Per-model breakdown */}
              {dimension === "model" && summary.by_model.length > 0 && (
                <div className="space-y-2">
                  <div className="text-2xs text-app-text-muted font-mono uppercase tracking-wider">
                    按模型拆分
                  </div>
                  <div className="border border-app-border bg-[var(--app-cmd-bg)] px-4 py-3 space-y-2.5">
                    <style>{`
                      @keyframes barGrow {
                        from { transform: scaleX(0); }
                        to   { transform: scaleX(1); }
                      }
                      @keyframes fadeUp {
                        from { opacity: 0; transform: translateY(4px); }
                        to   { opacity: 1; transform: translateY(0); }
                      }
                    `}</style>
                    {summary.by_model.map((m, i) => {
                      const displayPct = Math.round(m.percentage);
                      const barPct = Math.max(m.percentage, 0.3);
                      const total = m.tokens_in + m.tokens_out;
                      const delay = `${i * 0.1}s`;
                      return (
                        <div
                          key={m.model}
                          className="flex items-center gap-3"
                          style={{ animation: `fadeUp 0.4s ease-out ${delay} both` }}
                        >
                          {/* model name */}
                          <span className="text-xs text-app-text font-mono truncate shrink-0"
                            style={{ width: '140px' }}>
                            {m.model}
                          </span>
                          {/* bar */}
                          <div className="flex-1 h-4 bg-[var(--app-bg)] overflow-hidden relative">
                            <div
                              className="absolute inset-y-0 left-0 origin-left"
                              style={{
                                width: `${barPct}%`,
                                animation: `barGrow 0.5s cubic-bezier(0.22, 1, 0.36, 1) ${delay} both`,
                                background: `linear-gradient(90deg, color-mix(in srgb, var(--app-accent) 50%, transparent), var(--app-accent))`,
                                boxShadow: `1px 0 8px var(--app-glow)`,
                              }}
                            />
                          </div>
                          {/* stats */}
                          <div className="flex items-center gap-2 shrink-0">
                            <span className="text-2xs font-mono font-semibold tabular-nums w-7 text-right"
                              style={{ color: 'var(--app-accent)' }}>
                              {displayPct}%
                            </span>
                            <span className="text-2xs text-app-text-dim font-mono whitespace-nowrap">
                              入{formatTokens(m.tokens_in)} 出{formatTokens(m.tokens_out)}
                            </span>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}

              {/* Per-project breakdown */}
              {dimension === "project" && hasProjectData && (
                <div className="space-y-2">
                  <div className="text-2xs text-app-text-muted font-mono uppercase tracking-wider">
                    按项目拆分
                  </div>
                  <div className="border border-app-border bg-[var(--app-cmd-bg)] px-4 py-3 space-y-1">
                    <style>{`
                      @keyframes barGrow {
                        from { transform: scaleX(0); }
                        to   { transform: scaleX(1); }
                      }
                      @keyframes fadeUp {
                        from { opacity: 0; transform: translateY(4px); }
                        to   { opacity: 1; transform: translateY(0); }
                      }
                    `}</style>
                    {projectUsage.map((pu, i) => {
                      const displayPct = Math.round(pu.percentage);
                      const barPct = Math.max(pu.percentage, 0.3);
                      const label = projectDisplayLabel(pu, projectUsage);
                      const tooltip = projectTooltip(pu);
                      const total = pu.tokens_in + pu.tokens_out;
                      const delay = `${i * 0.1}s`;
                      const projKey = pu.project_path ?? "__unlinked__";
                      const isExpanded = expandedProject === projKey;
                      const hasModels = pu.models.length > 0;

                      return (
                        <div key={projKey} style={{ animation: `fadeUp 0.4s ease-out ${delay} both` }}>
                          <div
                            className={`flex items-center gap-3 py-1 ${hasModels ? "cursor-pointer hover:bg-[var(--app-hover)]" : ""}`}
                            onClick={() => {
                              if (hasModels) {
                                setExpandedProject(isExpanded ? null : projKey);
                              }
                            }}
                          >
                            {/* expand arrow */}
                            <span className="w-4 shrink-0 flex justify-center">
                              {hasModels && (
                                isExpanded
                                  ? <ChevronDown size={12} className="text-app-text-dim" />
                                  : <ChevronRight size={12} className="text-app-text-dim" />
                              )}
                            </span>
                            {/* project name */}
                            <span
                              className="text-xs text-app-text font-mono truncate shrink-0"
                              style={{ width: "140px" }}
                              title={tooltip}
                            >
                              {label}
                            </span>
                            {/* bar */}
                            <div className="flex-1 h-4 bg-[var(--app-bg)] overflow-hidden relative">
                              <div
                                className="absolute inset-y-0 left-0 origin-left"
                                style={{
                                  width: `${barPct}%`,
                                  animation: `barGrow 0.5s cubic-bezier(0.22, 1, 0.36, 1) ${delay} both`,
                                  background: `linear-gradient(90deg, color-mix(in srgb, var(--app-accent) 50%, transparent), var(--app-accent))`,
                                  boxShadow: `1px 0 8px var(--app-glow)`,
                                }}
                              />
                            </div>
                            {/* stats */}
                            <div className="flex items-center gap-2 shrink-0">
                              <span
                                className="text-2xs font-mono font-semibold tabular-nums w-7 text-right"
                                style={{ color: "var(--app-accent)" }}
                              >
                                {displayPct}%
                              </span>
                              <span className="text-2xs text-app-text-dim font-mono whitespace-nowrap">
                                入{formatTokens(pu.tokens_in)} 出{formatTokens(pu.tokens_out)}
                              </span>
                            </div>
                          </div>

                          {/* Drill-down: models within this project */}
                          {isExpanded && hasModels && (
                            <div className="ml-7 mt-1 mb-1 border-l border-app-border pl-4 space-y-1.5">
                              {pu.models.map((m, mi) => {
                                const mPct = Math.round(m.percentage);
                                const mBarPct = Math.max(m.percentage, 1.0);
                                return (
                                  <div
                                    key={m.model}
                                    className="flex items-center gap-2 text-2xs"
                                    style={{ animation: `fadeUp 0.3s ease-out ${mi * 0.05}s both` }}
                                  >
                                    <span className="text-app-text-dim font-mono truncate w-24 shrink-0">
                                      {m.model}
                                    </span>
                                    <div className="flex-1 h-2.5 bg-[var(--app-bg)] overflow-hidden relative">
                                      <div
                                        className="absolute inset-y-0 left-0 origin-left"
                                        style={{
                                          width: `${mBarPct}%`,
                                          background: `linear-gradient(90deg, color-mix(in srgb, var(--app-accent) 30%, transparent), color-mix(in srgb, var(--app-accent) 60%, transparent))`,
                                        }}
                                      />
                                    </div>
                                    <span
                                      className="font-mono font-semibold tabular-nums w-6 text-right"
                                      style={{ color: "var(--app-accent)" }}
                                    >
                                      {mPct}%
                                    </span>
                                    <span className="text-app-text-muted font-mono whitespace-nowrap">
                                      入{formatTokens(m.tokens_in)} 出{formatTokens(m.tokens_out)}
                                    </span>
                                  </div>
                                );
                              })}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}

              {dimension === "project" && !hasProjectData && (
                <div className="text-center py-4 text-xs text-app-text-muted font-mono">
                  暂无项目用量数据
                  <br />
                  <span className="text-2xs text-app-text-dim mt-1 block">
                    在项目目录使用 <code className="bg-[var(--app-cmd-bg)] px-1">ai claude</code> 启动后自动归因
                  </span>
                </div>
              )}

              {/* Daily trend bar chart */}
              {daily.length > 0 && (
                <div className="space-y-2">
                  <div className="text-2xs text-app-text-muted font-mono uppercase tracking-wider">
                    {period === "today" ? "今日趋势" : period === "week" ? "近 7 天趋势" : "近 30 天趋势"}
                  </div>
                  <div className="border border-app-border bg-[var(--app-cmd-bg)] px-4 pt-4 pb-2">
                    <div className="flex items-end justify-center gap-2 h-28">
                      {daily.map((d, i) => {
                        const maxVal = Math.max(...daily.map((x) => x.tokens_in + x.tokens_out), 1);
                        const h = Math.max(((d.tokens_in + d.tokens_out) / maxVal) * 100, 3);
                        return (
                          <div key={i} className="flex-1 flex flex-col items-center gap-1.5 h-full justify-end max-w-[48px]">
                            <span className="text-2xs text-app-text-dim font-mono tabular-nums leading-none">
                              {formatTokens(d.tokens_in + d.tokens_out)}
                            </span>
                            <div
                              className="w-full transition-all duration-300 hover:!opacity-80"
                              style={{
                                height: `${h}%`,
                                minHeight: h > 0 ? '3px' : '0',
                                background: `linear-gradient(to top, var(--app-accent), color-mix(in srgb, var(--app-accent) 40%, transparent))`,
                                boxShadow: `0 -2px 6px var(--app-glow)`,
                              }}
                              title={`${d.date}: ${d.tokens_in + d.tokens_out} tokens`}
                            />
                            <span className="text-2xs text-app-text-muted font-mono leading-none">
                              {formatDayLabel(d.date)}
                            </span>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                </div>
              )}
            </>
          ) : (
            <div className="flex flex-col items-center gap-2 text-center py-8 text-sm text-app-text-muted font-mono">
              {loading ? (
                <div className="w-4 h-4 border-2 border-app-border border-t-app-accent animate-spin rounded-full" />
              ) : (
                <BarChart3 size={20} className="opacity-30" />
              )}
              <span>{loading ? "加载中..." : "暂无用量数据"}</span>
              <br />
              <span className="text-2xs text-app-text-dim mt-1 block">
                在设置中开启 Token 用量追踪，使用 AI CLI 后数据自动记录
              </span>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-4 py-2.5 border-t border-app-border bg-[var(--app-subtle)] flex items-center justify-between">
          <button
            onClick={refresh}
            className="text-xs text-app-text-dim hover:text-app-text font-mono transition-colors"
          >
            刷新
          </button>
          <button
            onClick={onClose}
            className="px-4 py-1 text-xs font-mono text-app-text-dim hover:text-app-text
              border border-app-border bg-[var(--app-input)] hover:bg-[var(--app-hover)]
              transition-colors"
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}
