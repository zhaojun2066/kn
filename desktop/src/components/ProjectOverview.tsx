import React, { useState, useEffect, useRef, useCallback } from "react";
import type { ProjectInfo, SessionInfo, ProfileSummary, CliCounts, OverviewResources, CliConfigStatus, ProjectOverviewData } from "../lib/types";
import { CliBadge } from "./common/CliBadge";
import { CLI_HEX_COLORS } from "../lib/cli-constants";
import { relativeTime } from "../lib/time-utils";

// ── Types ────────────────────────────────────────────────────

interface ProjectOverviewProps {
  project: ProjectInfo;
  overviewData: ProjectOverviewData | null;
  overviewLoading: boolean;
  profiles: ProfileSummary[];
  onResumeSession: (session: SessionInfo) => void;
  onRunProfile: (name: string, cli: string) => void;
  onSplitProfile?: (name: string, cli: string) => void;
  onSetDefaultProfile: (name: string) => void;
}

// ── Shared helpers ───────────────────────────────────────────

const CLI_KEYS = ["claude", "codex", "qoder"] as const;
type CliKey = (typeof CLI_KEYS)[number];

const CLI_LABEL: Record<CliKey, string> = { claude: "Claude", codex: "Codex", qoder: "Qoder" };

function cliColor(cli: string): string {
  return CLI_HEX_COLORS[cli as CliKey] || "#6B7280";
}

// ── Sub-component: MetricCards ───────────────────────────────

interface MetricCardsProps {
  sessions: CliCounts;
  resources: OverviewResources;
}

const METRICS = [
  { key: "sessions", label: "Sessions", icon: "◉" } as const,
  { key: "skills", label: "Skills", icon: "⬡" } as const,
  { key: "plugins", label: "Plugins", icon: "⬢" } as const,
  { key: "commands", label: "Commands", icon: "⌘" } as const,
  { key: "agents", label: "Agents", icon: "◆" } as const,
];

function OverviewMetricCards({ sessions, resources }: MetricCardsProps) {
  const data: Record<string, CliCounts> = { sessions, ...resources };

  return (
    <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(5, 1fr)" }}>
      {METRICS.map(({ key, label, icon }) => {
        const counts = data[key];
        const values = CLI_KEYS.map((k) => counts[k]);
        const maxVal = Math.max(...values, 1); // avoid div-by-zero

        return (
          <div
            key={key}
            className="border border-app-border bg-app-sidebar p-3 flex flex-col gap-2.5
              transition-colors duration-fast hover:bg-[var(--app-hover)]"
          >
            {/* Header */}
            <div className="flex items-center justify-between">
              <span className="text-2xs font-mono text-app-text-muted tracking-wider uppercase">
                {label}
              </span>
              <span className="text-2xs text-app-text-dim opacity-40">{icon}</span>
            </div>

            {/* Big number */}
            <div className="text-xl font-mono font-semibold text-app-text tabular-nums leading-none">
              {counts.total}
            </div>

            {/* Per-CLI horizontal bar chart */}
            <div className="flex flex-col gap-1">
              {CLI_KEYS.map((cli) => {
                const val = counts[cli];
                const pct = maxVal > 0 ? (val / maxVal) * 100 : 0;
                const color = cliColor(cli);
                return (
                  <div key={cli} className="flex items-center gap-1.5">
                    {/* Label */}
                    <span
                      className="text-2xs font-mono w-10 shrink-0 text-right tabular-nums"
                      style={{ color: val > 0 ? color : "var(--app-text-muted)", opacity: val > 0 ? 0.85 : 0.4 }}
                    >
                      {CLI_LABEL[cli]}
                    </span>
                    {/* Bar track */}
                    <div className="flex-1 h-2 bg-[var(--app-border-light)] overflow-hidden">
                      {/* Bar fill */}
                      <div
                        className="h-full transition-all duration-300 ease-out"
                        style={{
                          width: `${Math.max(pct, val > 0 ? 4 : 0)}%`,
                          backgroundColor: color,
                          opacity: val > 0 ? 0.75 : 0,
                        }}
                      />
                    </div>
                    {/* Value */}
                    <span className="text-2xs font-mono text-app-text-dim tabular-nums w-5 shrink-0 text-right">
                      {val}
                    </span>
                  </div>
                );
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ── Sub-component: SectionHeader ─────────────────────────────

function SectionHeader({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-3">
      <span className="text-2xs font-mono text-app-text-muted tracking-widest uppercase shrink-0">
        {label}
      </span>
      <div className="flex-1 h-px bg-app-border" />
    </div>
  );
}

// ── Sub-component: RecentSessions ────────────────────────────

interface RecentSessionsProps {
  sessions: SessionInfo[];
  loading: boolean;
  onResume: (session: SessionInfo) => void;
}

function OverviewRecentSessions({ sessions, loading, onResume }: RecentSessionsProps) {
  if (loading) {
    return (
      <div className="border border-app-border bg-app-sidebar">
        {Array.from({ length: 4 }).map((_, i) => (
          <div
            key={i}
            className="flex items-center gap-3 px-3 py-2 border-b border-app-border-light last:border-b-0 animate-pulse"
          >
            <div className="w-2 h-2 rounded-full bg-app-border" />
            <div className="w-10 h-4 bg-app-border rounded" />
            <div className="flex-1 h-4 bg-app-border rounded" />
            <div className="w-14 h-3 bg-app-border rounded" />
          </div>
        ))}
      </div>
    );
  }

  if (sessions.length === 0) {
    return (
      <div className="border border-app-border bg-app-sidebar p-6 text-center">
        <span className="text-xs font-mono text-app-text-muted">暂无会话记录</span>
        <div className="text-2xs font-mono text-app-text-dim mt-1">
          使用 Claude Code / Codex / Qoder 打开此项目后，会话将出现在这里
        </div>
      </div>
    );
  }

  return (
    <div className="border border-app-border bg-app-sidebar">
      {sessions.slice(0, 8).map((s, i) => {
        const title = s.title.length > 48 ? s.title.slice(0, 48) + "…" : s.title;
        return (
          <div
            key={s.sessionId}
            className={`flex items-center gap-3 px-3 py-2 border-b border-app-border-light
              last:border-b-0 transition-colors duration-fast group
              hover:bg-[var(--app-hover)]`}
          >
            {/* Left: CLI color indicator */}
            <div className="w-px h-5 rounded-full shrink-0" style={{ backgroundColor: cliColor(s.cli) }} />

            {/* CLI badge */}
            <CliBadge cli={s.cli} />

            {/* Title */}
            <span className="flex-1 min-w-0 text-xs font-mono text-app-text truncate">
              {title}
            </span>

            {/* Time + Resume */}
            <span className="text-2xs font-mono text-app-text-muted shrink-0 tabular-nums w-16 text-right">
              {relativeTime(s.timestamp)}
            </span>

            <button
              onClick={(e) => { e.stopPropagation(); onResume(s); }}
              className="shrink-0 px-2 py-0.5 text-2xs font-mono text-app-accent
                border border-app-border bg-transparent
                opacity-0 group-hover:opacity-100 transition-opacity duration-fast
                hover:bg-app-accent hover:text-[var(--app-bg)]"
              title="恢复会话"
            >
              ▶
            </button>
          </div>
        );
      })}
    </div>
  );
}

// ── Sub-component: ConfigMatrix ──────────────────────────────

interface ConfigMatrixProps {
  matrix: CliConfigStatus[];
}

/** Config file name per CLI. */
function configFileName(cli: string): string {
  return cli === "codex" ? "config.toml" : "settings.json";
}

function OverviewConfigMatrix({ matrix }: ConfigMatrixProps) {
  return (
    <div className="border border-app-border bg-app-sidebar overflow-x-auto">
      {/* Header row */}
      <div className="grid border-b border-app-border" style={{ gridTemplateColumns: "72px 1fr 1fr 1fr" }}>
        <div className="p-2.5" />
        {CLI_KEYS.map((cli) => (
          <div
            key={cli}
            className="p-2.5 text-center border-l border-app-border"
            style={{ borderBottomWidth: 2, borderBottomStyle: "solid", borderBottomColor: cliColor(cli) }}
          >
            <span className="text-xs font-mono font-semibold" style={{ color: cliColor(cli) }}>
              {CLI_LABEL[cli]}
            </span>
          </div>
        ))}
      </div>

      {/* Row 1: config directory */}
      <div className="grid border-b border-app-border-light" style={{ gridTemplateColumns: "72px 1fr 1fr 1fr" }}>
        <div className="p-2.5 flex items-center">
          <span className="text-2xs font-mono text-app-text-muted">目录</span>
        </div>
        {CLI_KEYS.map((cli) => {
          const config = matrix.find((c) => c.cli === cli);
          const ok = config?.dirExists ?? false;
          return (
            <div key={cli} className="p-2.5 flex items-center justify-center gap-2 border-l border-app-border">
              <StatusDot ok={ok} />
              <span className="text-2xs font-mono" style={{ color: ok ? "var(--app-text)" : "var(--app-text-muted)" }}>
                {config?.dirName ?? ""}
              </span>
            </div>
          );
        })}
      </div>

      {/* Row 2: config file */}
      <div className="grid" style={{ gridTemplateColumns: "72px 1fr 1fr 1fr" }}>
        <div className="p-2.5 flex items-center">
          <span className="text-2xs font-mono text-app-text-muted">配置文件</span>
        </div>
        {CLI_KEYS.map((cli) => {
          const config = matrix.find((c) => c.cli === cli);
          const ok = config?.hasConfig ?? false;
          const fname = configFileName(cli);
          return (
            <div key={cli} className="p-2.5 flex items-center justify-center gap-2 border-l border-app-border">
              <StatusDot ok={ok} />
              <span className="text-2xs font-mono" style={{ color: ok ? "var(--app-text)" : "var(--app-text-muted)" }}>
                {config?.dirName ?? ""}/{fname}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Helper: StatusDot ────────────────────────────────────────

function StatusDot({ ok }: { ok: boolean }) {
  return (
    <span
      className="inline-block w-2 h-2 rounded-full shrink-0"
      style={{ backgroundColor: ok ? "var(--app-accent)" : "var(--app-text-muted)", opacity: ok ? 1 : 0.25 }}
      title={ok ? "正常" : "不可用"}
      aria-label={ok ? "正常" : "不可用"}
      role="status"
    />
  );
}

// ── Main component ───────────────────────────────────────────

export function ProjectOverview({
  project,
  overviewData,
  overviewLoading,
  profiles,
  onResumeSession,
  onRunProfile,
  onSplitProfile,
  onSetDefaultProfile,
}: ProjectOverviewProps) {

  const defaultProfile = project.defaultProfile;
  const defaultProfileObj = profiles.find((p) => p.name === defaultProfile);

  // ── Picker state (replicates ProjectWorkspace header controls) ──
  const [showRunPicker, setShowRunPicker] = useState(false);
  const [showDefaultPicker, setShowDefaultPicker] = useState(false);
  const [focusedIdx, setFocusedIdx] = useState(0);
  const runRef = useRef<HTMLDivElement>(null);
  const defaultRef = useRef<HTMLDivElement>(null);

  // Close pickers on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (runRef.current && !runRef.current.contains(e.target as Node)) {
        setShowRunPicker(false);
      }
      if (defaultRef.current && !defaultRef.current.contains(e.target as Node)) {
        setShowDefaultPicker(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const handleRunDefault = useCallback((e: React.MouseEvent) => {
    if (defaultProfile && defaultProfileObj) {
      if (e.altKey && onSplitProfile) {
        onSplitProfile(defaultProfile, defaultProfileObj.cli_type || "claude");
      } else {
        onRunProfile(defaultProfile, defaultProfileObj.cli_type || "claude");
      }
    } else {
      setShowRunPicker((v) => !v);
      setShowDefaultPicker(false);
    }
  }, [defaultProfile, defaultProfileObj, onRunProfile, onSplitProfile]);

  const handleSelectProfile = useCallback((profile: ProfileSummary) => {
    onRunProfile(profile.name, profile.cli_type || "claude");
    setShowRunPicker(false);
  }, [onRunProfile]);

  const handleSelectDefault = useCallback((profile: ProfileSummary) => {
    onSetDefaultProfile(profile.name);
    setShowDefaultPicker(false);
  }, [onSetDefaultProfile]);

  const handlePickerKeyDown = useCallback((e: React.KeyboardEvent, mode: "run" | "default") => {
    if (e.key === "ArrowDown") { e.preventDefault(); setFocusedIdx((i) => Math.min(i + 1, profiles.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setFocusedIdx((i) => Math.max(i - 1, 0)); }
    else if (e.key === "Enter") { e.preventDefault();
      if (profiles[focusedIdx]) {
        if (mode === "run") handleSelectProfile(profiles[focusedIdx]);
        else handleSelectDefault(profiles[focusedIdx]);
      }
    }
    else if (e.key === "Escape") { setShowRunPicker(false); setShowDefaultPicker(false); }
  }, [profiles, focusedIdx, handleSelectProfile, handleSelectDefault]);

  return (
    <div className="flex-1 min-h-0 overflow-y-auto bg-[var(--app-bg)]">
      <div className="p-5 space-y-5 max-w-[960px]">
        {/* ── Project Identity + Controls ── */}
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0 flex-1">
            <h1 className="text-sm font-mono font-semibold text-app-text truncate">
              {project.name}
            </h1>
            <p className="text-2xs font-mono text-app-text-muted mt-0.5 truncate">
              {project.path}
            </p>
            {project.description && (
              <p className="text-2xs font-mono text-app-text-dim mt-1.5 line-clamp-2">
                {project.description}
              </p>
            )}
          </div>

          {/* Controls: Default profile + Run (replicates header bar) */}
          <div className="shrink-0 flex items-center gap-2">
            {/* Default profile picker */}
            <div ref={defaultRef} className="relative">
              <button
                onClick={() => { setShowDefaultPicker((v) => !v); setShowRunPicker(false); }}
                className="h-7 w-[180px] flex items-center gap-2 px-2 border border-app-border
                  bg-app-sidebar text-xs font-mono hover:bg-[var(--app-hover)] transition-colors"
              >
                <span className="text-app-text-muted shrink-0">默认</span>
                {defaultProfile ? (
                  <span className="text-app-accent truncate flex-1 text-left">{defaultProfile}</span>
                ) : (
                  <span className="text-app-text-dim truncate flex-1 text-left">未设置</span>
                )}
                <span className="text-app-text-dim shrink-0">▾</span>
              </button>
              {showDefaultPicker && profiles.length > 0 && (
                <div
                  className="absolute right-0 top-full mt-1 w-52 bg-app-sidebar border border-app-border
                    shadow-lg z-30 max-h-60 overflow-y-auto"
                  onKeyDown={(e) => handlePickerKeyDown(e, "default")}
                >
                  {profiles.map((p, i) => (
                    <button
                      key={p.name}
                      onClick={() => handleSelectDefault(p)}
                      className={`w-full flex items-center gap-1.5 px-2.5 py-1 text-left text-2xs font-mono
                        transition-colors duration-fast
                        ${i === focusedIdx ? "bg-[var(--app-accent)]/10 text-app-text" : "text-app-text-dim hover:bg-[var(--app-hover)]"}
                        ${p.name === defaultProfile ? "bg-[var(--app-accent)]/5" : ""}`}
                    >
                      <CliBadge cli={p.cli_type || "claude"} />
                      <span className="flex-1 truncate">{p.name}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* Run split button */}
            <div className="flex items-stretch h-7">
              <div className="relative group/run">
                <button
                  onClick={handleRunDefault}
                  className="h-7 flex items-center gap-1.5 px-3 text-xs font-mono
                    bg-app-accent text-[var(--app-bg)] hover:opacity-90 transition-opacity"
                  title={defaultProfile ? `Run with ${defaultProfile}` : "Select profile"}
                >
                  <span>▶</span>
                  <span>Run</span>
                </button>
                <div className="absolute top-full left-1/2 -translate-x-1/2 mt-1.5 px-2 py-1
                  bg-[var(--app-panel)] text-[var(--app-text)] text-2xs
                  border border-[var(--app-border)] shadow-dialog
                  whitespace-nowrap pointer-events-none
                  opacity-0 group-hover/run:opacity-100
                  transition-opacity duration-150 delay-700
                  group-hover/run:delay-700">
                  <span>在终端中运行</span>
                  <span className="text-[var(--app-text-muted)] ml-1">{navigator.userAgent.includes("Mac") ? "⌥+Click" : "Alt+Click"} 分屏运行</span>
                </div>
              </div>
              <div ref={runRef} className="relative">
                <button
                  onClick={() => { setShowRunPicker((v) => !v); setShowDefaultPicker(false); }}
                  className="h-7 px-1.5 text-xs font-mono
                    bg-app-accent text-[var(--app-bg)] hover:opacity-90 transition-opacity
                    border-l border-[var(--app-bg)]/20"
                >
                  ▾
                </button>
                {showRunPicker && profiles.length > 0 && (
                  <div
                    className="absolute right-0 top-full mt-1 w-52 bg-app-sidebar border border-app-border
                      shadow-lg z-30 max-h-60 overflow-y-auto"
                    onKeyDown={(e) => handlePickerKeyDown(e, "run")}
                  >
                    {profiles.map((p, i) => (
                      <button
                        key={p.name}
                        onClick={() => handleSelectProfile(p)}
                        className={`w-full flex items-center gap-1.5 px-2.5 py-1 text-left text-2xs font-mono
                          transition-colors duration-fast
                          ${i === focusedIdx ? "bg-[var(--app-accent)]/10 text-app-text" : "text-app-text-dim hover:bg-[var(--app-hover)]"}
                          ${p.name === defaultProfile ? "bg-[var(--app-accent)]/5" : ""}`}
                      >
                        <CliBadge cli={p.cli_type || "claude"} />
                        <span className="flex-1 truncate">{p.name}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>

        {/* ── Divider ── */}
        <div className="h-px bg-app-border" />

        {/* ── Metrics ── */}
        <SectionHeader label="Metrics" />
        {overviewLoading && !overviewData ? (
          <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(5, 1fr)" }}>
            {Array.from({ length: 5 }).map((_, i) => (
              <div key={i} className="border border-app-border bg-app-sidebar p-3 animate-pulse">
                <div className="h-3 w-14 bg-app-border rounded mb-3" />
                <div className="h-6 w-10 bg-app-border rounded mb-2" />
                <div className="space-y-1">
                  <div className="h-2.5 w-full bg-app-border rounded" />
                  <div className="h-2.5 w-3/4 bg-app-border rounded" />
                  <div className="h-2.5 w-1/2 bg-app-border rounded" />
                </div>
              </div>
            ))}
          </div>
        ) : overviewData ? (
          <OverviewMetricCards sessions={overviewData.sessions} resources={overviewData.resources} />
        ) : (
          <div className="border border-app-border bg-app-sidebar p-6 text-center">
            <span className="text-xs font-mono text-app-text-muted">无法加载项目指标</span>
          </div>
        )}

        {/* ── Recent Sessions ── */}
        <SectionHeader label="Recent Sessions" />
        <OverviewRecentSessions
          sessions={overviewData?.recentSessions ?? []}
          loading={overviewLoading}
          onResume={onResumeSession}
        />

        {/* ── Config Status ── */}
        <SectionHeader label="Config Status" />
        {overviewData ? (
          <OverviewConfigMatrix matrix={overviewData.configMatrix} />
        ) : (
          <div className="border border-app-border bg-app-sidebar animate-pulse">
            <div className="grid grid-cols-4 gap-2 p-3">
              {Array.from({ length: 6 }).map((_, i) => (
                <div key={i} className="h-4 bg-app-border rounded col-span-4" />
              ))}
            </div>
          </div>
        )}

        {/* Bottom spacer for comfortable scroll */}
        <div className="h-2" />
      </div>
    </div>
  );
}
