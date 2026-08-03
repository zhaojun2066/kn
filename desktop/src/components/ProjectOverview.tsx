import React, { useState, useEffect, useRef, useCallback } from "react";
import { createPortal } from "react-dom";
import type { ProjectInfo, SessionInfo, ProfileSummary, CliCounts, OverviewResources, CliConfigStatus, ProjectOverviewData, ProjectVerifyConfig, ProjectVerifyCommand } from "../lib/types";
import { CliBadge } from "./common/CliBadge";
import { CLI_HEX_COLORS } from "../lib/cli-constants";
import { relativeTime } from "../lib/time-utils";

// ── Types ────────────────────────────────────────────────────

interface ProjectOverviewProps {
  project: ProjectInfo;
  overviewData: ProjectOverviewData | null;
  overviewLoading: boolean;
  profiles: ProfileSummary[];
  onResumeSession: (session: SessionInfo, profileName: string) => void;
  onRunProfile: (name: string, cli: string) => void;
  onSplitProfile?: (name: string, cli: string) => void;
  onSetDefaultProfile: (name: string) => void;
  onUpdateVerifyConfig: (verify: ProjectVerifyConfig | null) => Promise<void> | void;
  onPreviewVerifyConfig: (projectName: string) => Promise<ProjectVerifyConfig | null>;
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
  profiles: ProfileSummary[];
  onResume: (session: SessionInfo, profileName: string) => void;
}

function OverviewRecentSessions({ sessions, loading, profiles, onResume }: RecentSessionsProps) {
  const [sessionToResume, setSessionToResume] = useState<SessionInfo | null>(null);
  const [selectedProfile, setSelectedProfile] = useState("");
  const compatibleProfiles = sessionToResume
    ? profiles.filter((profile) => profile.cli_type === sessionToResume.cli)
    : [];

  const openResumePicker = (session: SessionInfo) => {
    setSessionToResume(session);
    setSelectedProfile("");
  };
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
              onClick={(e) => { e.stopPropagation(); openResumePicker(s); }}
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
      {sessionToResume && createPortal(
        <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50">
          <div className="w-[360px] border border-app-border bg-app-panel p-5 shadow-dialog">
            <h3 className="text-sm font-medium text-app-text">选择 Profile</h3>
            <p className="mt-2 text-xs text-app-text-muted">恢复会话前请选择 {sessionToResume.cli} 的 Profile。</p>
            <select value={selectedProfile} onChange={(event) => setSelectedProfile(event.target.value)} className="mt-4 w-full border border-app-border bg-app-input px-2 py-2 text-sm text-app-text">
              <option value="">请选择 Profile</option>
              {compatibleProfiles.map((profile) => <option key={profile.name} value={profile.name}>{profile.name}</option>)}
            </select>
            <div className="mt-5 flex justify-end gap-2">
              <button onClick={() => setSessionToResume(null)} className="px-3 py-1.5 text-xs text-app-text-muted">取消</button>
              <button disabled={!selectedProfile} onClick={() => { onResume(sessionToResume, selectedProfile); setSessionToResume(null); }} className="px-3 py-1.5 text-xs text-app-bg bg-app-accent disabled:opacity-40">恢复会话</button>
            </div>
          </div>
        </div>, document.body,
      )}
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

function verifyCommandSummary(command?: ProjectVerifyCommand): string {
  if (!command) return "未配置";
  if (!command.enabled) return command.command ? `已禁用 · ${command.command}` : "已禁用";
  return command.command || "未配置";
}

function resolveVerifyEnvironmentName(verify?: ProjectVerifyConfig): string {
  if (!verify) return "default";
  const preferred = verify.defaultEnvironment || "default";
  if (verify.environments?.[preferred]) return preferred;
  return verify.environments?.default ? "default" : preferred;
}

function normalizeVerifyConfig(verify: ProjectVerifyConfig | null): ProjectVerifyConfig | null {
  if (!verify) return null;
  const envName = resolveVerifyEnvironmentName(verify);
  const env = verify.environments?.[envName] ?? {};
  return {
    defaultEnvironment: "default",
    environments: {
      default: {
        ...(env.build ? { build: env.build } : {}),
        ...(env.test ? { test: env.test } : {}),
      },
    },
  };
}

function configsEqual(left: ProjectVerifyConfig | null, right: ProjectVerifyConfig | null): boolean {
  return JSON.stringify(normalizeVerifyConfig(left)) === JSON.stringify(normalizeVerifyConfig(right));
}

function ProjectVerifyCard({
  project,
  onEdit,
  onPreview,
}: {
  project: ProjectInfo;
  onEdit: () => void;
  onPreview: (projectName: string) => Promise<ProjectVerifyConfig | null>;
}) {
  const [autoPreview, setAutoPreview] = useState<ProjectVerifyConfig | null>(null);
  const [loadingPreview, setLoadingPreview] = useState(false);
  const [previewError, setPreviewError] = useState(false);
  const displayConfig = project.verify ?? autoPreview;
  const envName = resolveVerifyEnvironmentName(displayConfig ?? undefined);
  const env = displayConfig?.environments?.[envName];
  const source = project.verify ? "桌面端配置" : "自动识别";

  useEffect(() => {
    if (project.verify) {
      setAutoPreview(null);
      setLoadingPreview(false);
      setPreviewError(false);
      return;
    }
    let cancelled = false;
    setLoadingPreview(true);
    setPreviewError(false);
    onPreview(project.name)
      .then((config) => {
        if (cancelled) return;
        setAutoPreview(config);
      })
      .catch(() => {
        if (cancelled) return;
        setAutoPreview(null);
        setPreviewError(true);
      })
      .finally(() => {
        if (!cancelled) setLoadingPreview(false);
      });
    return () => {
      cancelled = true;
    };
  }, [onPreview, project.name, project.verify]);

  const summary = (command?: ProjectVerifyCommand) => {
    if (loadingPreview && !project.verify) return "正在自动识别...";
    if (previewError && !project.verify) return "自动识别失败";
    return verifyCommandSummary(command);
  };

  return (
    <div className="border border-app-border bg-app-sidebar p-3 space-y-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-xs font-mono font-semibold text-app-text">验证</div>
          <div className="text-2xs font-mono text-app-text-muted mt-0.5">
            环境 {envName} · {source}
          </div>
        </div>
        <button
          onClick={onEdit}
          className="px-2 py-1 text-2xs font-mono border border-app-border text-app-text-muted hover:text-app-text hover:bg-[var(--app-hover)]"
        >
          编辑配置
        </button>
      </div>
      <div className="grid grid-cols-2 gap-2">
        <div className="border border-app-border-light p-2">
          <div className="text-2xs font-mono text-app-text-muted mb-1">构建</div>
          <div className="text-2xs font-mono text-app-text truncate" title={env?.build?.command}>
            {summary(env?.build)}
          </div>
        </div>
        <div className="border border-app-border-light p-2">
          <div className="text-2xs font-mono text-app-text-muted mb-1">测试</div>
          <div className="text-2xs font-mono text-app-text truncate" title={env?.test?.command}>
            {summary(env?.test)}
          </div>
        </div>
      </div>
    </div>
  );
}

function ProjectVerifyConfigEditor({
  project,
  onClose,
  onSave,
  onPreview,
}: {
  project: ProjectInfo;
  onClose: () => void;
  onSave: (verify: ProjectVerifyConfig | null) => Promise<void> | void;
  onPreview: (projectName: string) => Promise<ProjectVerifyConfig | null>;
}) {
  const [previewConfig, setPreviewConfig] = useState<ProjectVerifyConfig | null>(project.verify ?? null);
  const [buildEnabled, setBuildEnabled] = useState(true);
  const [buildCommand, setBuildCommand] = useState("");
  const [buildTimeout, setBuildTimeout] = useState("300");
  const [buildParserHint, setBuildParserHint] = useState("");
  const [buildTaskTypeHint, setBuildTaskTypeHint] = useState("");
  const [buildReportHints, setBuildReportHints] = useState("");
  const [testEnabled, setTestEnabled] = useState(true);
  const [testCommand, setTestCommand] = useState("");
  const [testTimeout, setTestTimeout] = useState("600");
  const [testParserHint, setTestParserHint] = useState("");
  const [testTaskTypeHint, setTestTaskTypeHint] = useState("");
  const [testReportHints, setTestReportHints] = useState("");
  const [loadingPlan, setLoadingPlan] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const source = project.verify ? "桌面端配置" : "自动识别";

  const parseTimeout = (value: string, fallback: number) => {
    const parsed = Number.parseInt(value, 10);
    if (!Number.isFinite(parsed)) return fallback;
    return Math.max(1, Math.min(900, parsed));
  };
  const parseTaskTypeHint = (value: string): ProjectVerifyCommand["taskTypeHint"] => {
    const normalized = value.trim().toLowerCase();
    return ["compile", "test", "package", "build", "run", "lint", "custom"].includes(normalized)
      ? normalized as ProjectVerifyCommand["taskTypeHint"]
      : undefined;
  };

  const applyConfig = useCallback((config: ProjectVerifyConfig | null) => {
    const envName = resolveVerifyEnvironmentName(config ?? undefined);
    const env = config?.environments?.[envName];
    setBuildEnabled(env?.build?.enabled ?? true);
    setBuildCommand(env?.build?.command ?? "");
    setBuildTimeout(String(env?.build?.timeoutSeconds ?? 300));
    setBuildParserHint(env?.build?.parserHint ?? "");
    setBuildTaskTypeHint(env?.build?.taskTypeHint ?? "");
    setBuildReportHints((env?.build?.reportHints ?? []).join(", "));
    setTestEnabled(env?.test?.enabled ?? true);
    setTestCommand(env?.test?.command ?? "");
    setTestTimeout(String(env?.test?.timeoutSeconds ?? 600));
    setTestParserHint(env?.test?.parserHint ?? "");
    setTestTaskTypeHint(env?.test?.taskTypeHint ?? "");
    setTestReportHints((env?.test?.reportHints ?? []).join(", "));
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoadingPlan(true);
    setError(null);
    onPreview(project.name)
      .then((config) => {
        if (cancelled) return;
        setPreviewConfig(config);
        applyConfig(config);
      })
      .catch((e) => {
        if (cancelled) return;
        setPreviewConfig(project.verify ?? null);
        applyConfig(project.verify ?? null);
        setError(`读取自动识别命令失败: ${e}`);
      })
      .finally(() => {
        if (!cancelled) setLoadingPlan(false);
      });
    return () => {
      cancelled = true;
    };
  }, [applyConfig, onPreview, project.name, project.verify]);

  const configFromForm = (): ProjectVerifyConfig | null => {
    const build = buildCommand.trim()
      ? { command: buildCommand.trim(), enabled: buildEnabled, timeoutSeconds: parseTimeout(buildTimeout, 300), parserHint: buildParserHint.trim() || undefined, taskTypeHint: parseTaskTypeHint(buildTaskTypeHint), reportHints: buildReportHints.split(",").map((v) => v.trim()).filter(Boolean) }
      : undefined;
    const test = testCommand.trim()
      ? { command: testCommand.trim(), enabled: testEnabled, timeoutSeconds: parseTimeout(testTimeout, 600), parserHint: testParserHint.trim() || undefined, taskTypeHint: parseTaskTypeHint(testTaskTypeHint), reportHints: testReportHints.split(",").map((v) => v.trim()).filter(Boolean) }
      : undefined;
    if (!build && !test) return null;
    return {
      defaultEnvironment: "default",
      environments: {
        default: {
          ...(build ? { build } : {}),
          ...(test ? { test } : {}),
        },
      },
    };
  };

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      const verify = configFromForm();
      if (!verify) {
        await onSave(null);
        onClose();
        return;
      }
      await onSave(!project.verify && configsEqual(verify, previewConfig) ? null : verify);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleReset = async () => {
    setSaving(true);
    setError(null);
    try {
      await onSave(null);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[560px] max-w-[calc(100vw-32px)] border border-app-border bg-app-panel shadow-lg">
        <div className="px-4 py-3 border-b border-app-border">
          <div className="text-sm font-mono font-semibold text-app-text">验证配置</div>
          <div className="text-2xs font-mono text-app-text-muted mt-1">
            {project.name} · default · {loadingPlan ? "读取中" : source}
          </div>
        </div>
        <div className="p-4 space-y-4">
          <VerifyCommandEditor
            title="构建"
            enabled={buildEnabled}
            command={buildCommand}
            timeout={buildTimeout}
            onEnabledChange={setBuildEnabled}
            onCommandChange={setBuildCommand}
            onTimeoutChange={setBuildTimeout}
            parserHint={buildParserHint}
            taskTypeHint={buildTaskTypeHint}
            reportHints={buildReportHints}
            onParserHintChange={setBuildParserHint}
            onTaskTypeHintChange={setBuildTaskTypeHint}
            onReportHintsChange={setBuildReportHints}
            placeholder={loadingPlan ? "正在读取当前验证计划..." : "未识别到构建命令，可手动填写"}
          />
          <VerifyCommandEditor
            title="测试"
            enabled={testEnabled}
            command={testCommand}
            timeout={testTimeout}
            onEnabledChange={setTestEnabled}
            onCommandChange={setTestCommand}
            onTimeoutChange={setTestTimeout}
            parserHint={testParserHint}
            taskTypeHint={testTaskTypeHint}
            reportHints={testReportHints}
            onParserHintChange={setTestParserHint}
            onTaskTypeHintChange={setTestTaskTypeHint}
            onReportHintsChange={setTestReportHints}
            placeholder={loadingPlan ? "正在读取当前验证计划..." : "未识别到测试命令，可手动填写"}
          />
          {error && <div className="text-2xs font-mono text-red-400">{error}</div>}
        </div>
        <div className="px-4 py-3 border-t border-app-border flex items-center justify-between">
          <button onClick={handleReset} disabled={saving} className="text-2xs font-mono text-app-text-muted hover:text-app-text">
            恢复自动识别
          </button>
          <div className="flex items-center gap-2">
            <button onClick={onClose} disabled={saving} className="px-3 py-1.5 text-2xs font-mono border border-app-border text-app-text-muted hover:text-app-text">
              取消
            </button>
            <button onClick={handleSave} disabled={saving} className="px-3 py-1.5 text-2xs font-mono bg-app-accent text-[var(--app-bg)]">
              保存
            </button>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}

function VerifyCommandEditor({
  title,
  enabled,
  command,
  timeout,
  onEnabledChange,
  onCommandChange,
  onTimeoutChange,
  parserHint,
  taskTypeHint,
  reportHints,
  onParserHintChange,
  onTaskTypeHintChange,
  onReportHintsChange,
  placeholder,
}: {
  title: string;
  enabled: boolean;
  command: string;
  timeout: string;
  onEnabledChange: (value: boolean) => void;
  onCommandChange: (value: string) => void;
  onTimeoutChange: (value: string) => void;
  parserHint: string;
  taskTypeHint: string;
  reportHints: string;
  onParserHintChange: (value: string) => void;
  onTaskTypeHintChange: (value: string) => void;
  onReportHintsChange: (value: string) => void;
  placeholder: string;
}) {
  return (
    <div className="border border-app-border p-3 space-y-2">
      <label className="flex items-center gap-2 text-xs font-mono text-app-text">
        <input type="checkbox" checked={enabled} onChange={(e) => onEnabledChange(e.target.checked)} />
        {title}
      </label>
      <input
        value={command}
        onChange={(e) => onCommandChange(e.target.value)}
        placeholder={placeholder}
        className="w-full px-2 py-1.5 bg-app-bg border border-app-border text-xs font-mono text-app-text"
      />
      <label className="flex items-center gap-2 text-2xs font-mono text-app-text-muted">
        超时秒数
        <input
          value={timeout}
          onChange={(e) => onTimeoutChange(e.target.value)}
          className="w-20 px-2 py-1 bg-app-bg border border-app-border text-app-text"
        />
      </label>
      <div className="grid grid-cols-2 gap-2">
        <input value={parserHint} onChange={(e) => onParserHintChange(e.target.value)} placeholder="Parser 提示（可选）" className="px-2 py-1 bg-app-bg border border-app-border text-2xs font-mono text-app-text" />
        <input value={taskTypeHint} onChange={(e) => onTaskTypeHintChange(e.target.value)} placeholder="任务类型（test/build…）" className="px-2 py-1 bg-app-bg border border-app-border text-2xs font-mono text-app-text" />
      </div>
      <input value={reportHints} onChange={(e) => onReportHintsChange(e.target.value)} placeholder="报告路径提示（逗号分隔，可选）" className="w-full px-2 py-1 bg-app-bg border border-app-border text-2xs font-mono text-app-text" />
    </div>
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
  onUpdateVerifyConfig,
  onPreviewVerifyConfig,
}: ProjectOverviewProps) {

  const defaultProfile = project.defaultProfile;
  const defaultProfileObj = profiles.find((p) => p.name === defaultProfile);

  // ── Picker state (replicates ProjectWorkspace header controls) ──
  const [showRunPicker, setShowRunPicker] = useState(false);
  const [showDefaultPicker, setShowDefaultPicker] = useState(false);
  const [showVerifyEditor, setShowVerifyEditor] = useState(false);
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

        {/* ── Verification ── */}
        <SectionHeader label="Verification" />
        <ProjectVerifyCard
          project={project}
          onEdit={() => setShowVerifyEditor(true)}
          onPreview={onPreviewVerifyConfig}
        />
        {showVerifyEditor && (
          <ProjectVerifyConfigEditor
            project={project}
            onClose={() => setShowVerifyEditor(false)}
            onSave={onUpdateVerifyConfig}
            onPreview={onPreviewVerifyConfig}
          />
        )}

        {/* ── Recent Sessions ── */}
        <SectionHeader label="Recent Sessions" />
        <OverviewRecentSessions
          sessions={overviewData?.recentSessions ?? []}
          loading={overviewLoading}
          profiles={profiles}
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
